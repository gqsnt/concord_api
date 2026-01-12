mod common;
use common::*;

use concord_core::prelude::*;
use concord_macros::api;

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

    let (transport, recorded) = MockTransport::new(vec![MockReply::ok_json(json_bytes(&()))]);
    let api = ApiPrefixDefault::new_with_transport( transport);

    let _ = api.execute(endpoints::Ping::new()).await.unwrap();
    let host0 = recorded.lock().unwrap()[0].url.host_str().unwrap().to_string();
    assert_eq!(host0, "euw1.api.example.com");
    assert!(!host0.starts_with("api.euw1"));

    let (transport, recorded) = MockTransport::new(vec![MockReply::ok_json(json_bytes(&()))]);
    let api = ApiPrefixDefault::new_with_transport(transport);

    let _ = api.execute(endpoints::Ping::new().region(Region::NA)).await.unwrap();
    let host1 = recorded.lock().unwrap()[0].url.host_str().unwrap().to_string();
    assert_eq!(host1, "na1.api.example.com");
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

    // sub=None => "api.example.com" (pas ".api.example.com")
    let (transport, recorded) = MockTransport::new(vec![MockReply::ok_json(json_bytes(&()))]);
    let api = ApiPrefixOpt::new_with_transport( transport);

    let _ = api.execute(endpoints::Ping::new()).await.unwrap();
    let host0 = recorded.lock().unwrap()[0].url.host_str().unwrap().to_string();
    assert_eq!(host0, "api.example.com");

    // sub=Some("x") => "x.api.example.com"
    let (transport, recorded) = MockTransport::new(vec![MockReply::ok_json(json_bytes(&()))]);
    let api = ApiPrefixOpt::new_with_transport( transport);

    let _ = api.execute(endpoints::Ping::new().sub("x".to_string())).await.unwrap();
    let host1 = recorded.lock().unwrap()[0].url.host_str().unwrap().to_string();
    assert_eq!(host1, "x.api.example.com");
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
        let (transport, _recorded) = MockTransport::new(vec![]);
        let api = ApiPrefixInvalid::new_with_transport( transport);

        let err = api.execute(endpoints::Ping::new().region(Region::BadDot)).await.unwrap_err();
        match err {
            ApiClientError::InEndpoint { source, .. } => match *source {
                ApiClientError::InvalidHostLabel { reason, .. } => {
                    assert!(matches!(reason, concord_core::error::HostLabelInvalidReason::ContainsDot));
                }
                other => panic!("unexpected inner error: {other:?}"),
            },
            other => panic!("unexpected error: {other:?}"),
        }
    }

    // BadDashStart => StartsOrEndsDash
    {
        let (transport, _recorded) = MockTransport::new(vec![]);
        let api = ApiPrefixInvalid::new_with_transport( transport);

        let err = api.execute(endpoints::Ping::new().region(Region::BadDashStart)).await.unwrap_err();
        match err {
            ApiClientError::InEndpoint { source, .. } => match *source {
                ApiClientError::InvalidHostLabel { reason, .. } => {
                    assert!(matches!(reason, concord_core::error::HostLabelInvalidReason::StartsOrEndsDash));
                }
                other => panic!("unexpected inner error: {other:?}"),
            },
            other => panic!("unexpected error: {other:?}"),
        }
    }

    // BadUnderscore => InvalidByte
    {
        let (transport, _recorded) = MockTransport::new(vec![]);
        let api = ApiPrefixInvalid::new_with_transport( transport);

        let err = api.execute(endpoints::Ping::new().region(Region::BadUnderscore)).await.unwrap_err();
        match err {
            ApiClientError::InEndpoint { source, .. } => match *source {
                ApiClientError::InvalidHostLabel { reason, .. } => {
                    assert!(matches!(reason, concord_core::error::HostLabelInvalidReason::InvalidByte(_)));
                }
                other => panic!("unexpected inner error: {other:?}"),
            },
            other => panic!("unexpected error: {other:?}"),
        }
    }
}
