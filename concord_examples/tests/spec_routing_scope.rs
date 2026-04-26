use concord_core::prelude::*;
use concord_macros::api;
use concord_test_support::*;

#[derive(Clone, Debug)]
enum Region {
    Euw,
    NA,
    BadDot,
    BadDashStart,
    BadUnderscore,
}

impl core::fmt::Display for Region {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Region::Euw => f.write_str("euw1"),
            Region::NA => f.write_str("na1"),
            Region::BadDot => f.write_str("bad.name"),
            Region::BadDashStart => f.write_str("-bad"),
            Region::BadUnderscore => f.write_str("bad_name"),
        }
    }
}

#[tokio::test]
async fn scope_host_default_and_override_and_order() {
    api! {
        client ApiPrefixDefault {
            base https "example.com"
        }

        scope platform(region: Region = Region::Euw) {
            host [region, "api"]

            GET Ping
            -> Json<()>
            {
            }
        }
    }

    use api_prefix_default::*;

    {
        let (transport, h) = mock().reply(MockReply::ok_json(json_bytes(&()))).build();

        let api = ApiPrefixDefault::new_with_transport(transport);
        api.request(endpoints::platform::Ping::new())
            .execute()
            .await
            .unwrap();

        let reqs = h.recorded();
        assert_request(&reqs[0]).host("euw1.api.example.com");

        h.finish();
    }

    {
        let (transport, h) = mock().reply(MockReply::ok_json(json_bytes(&()))).build();

        let api = ApiPrefixDefault::new_with_transport(transport);
        api.request(endpoints::platform::Ping::new().region(Region::NA))
            .execute()
            .await
            .unwrap();

        let reqs = h.recorded();
        assert_request(&reqs[0]).host("na1.api.example.com");

        h.finish();
    }
}

#[tokio::test]
async fn scope_host_optional_label_omitted_without_double_dot() {
    api! {
        client ApiPrefixOpt {
            base https "example.com"
        }

        scope tenant(sub?: String) {
            host [sub, "api"]

            GET Ping
            -> Json<()>
            {
            }
        }
    }

    use api_prefix_opt::*;

    {
        let (transport, h) = mock().reply(MockReply::ok_json(json_bytes(&()))).build();

        let api = ApiPrefixOpt::new_with_transport(transport);
        api.request(endpoints::tenant::Ping::new())
            .execute()
            .await
            .unwrap();

        let reqs = h.recorded();
        assert_request(&reqs[0]).host("api.example.com");

        h.finish();
    }

    {
        let (transport, h) = mock().reply(MockReply::ok_json(json_bytes(&()))).build();

        let api = ApiPrefixOpt::new_with_transport(transport);
        api.request(endpoints::tenant::Ping::new().sub("x".to_string()))
            .execute()
            .await
            .unwrap();

        let reqs = h.recorded();
        assert_request(&reqs[0]).host("x.api.example.com");

        h.finish();
    }
}

#[tokio::test]
async fn scope_host_label_validation_errors() {
    api! {
        client ApiPrefixInvalid {
            base https "example.com"
        }

        scope platform(region: Region = Region::Euw) {
            host [region, "api"]

            GET Ping
            -> Json<()>
            {
            }
        }
    }

    use api_prefix_invalid::*;

    {
        let (transport, h) = mock().build();

        let api = ApiPrefixInvalid::new_with_transport(transport);
        let err = api
            .request(endpoints::platform::Ping::new().region(Region::BadDot))
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

    {
        let (transport, h) = mock().build();

        let api = ApiPrefixInvalid::new_with_transport(transport);
        let err = api
            .request(endpoints::platform::Ping::new().region(Region::BadDashStart))
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

    {
        let (transport, h) = mock().build();

        let api = ApiPrefixInvalid::new_with_transport(transport);
        let err = api
            .request(endpoints::platform::Ping::new().region(Region::BadUnderscore))
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

#[tokio::test]
async fn scope_path_concat_percent_encoding() {
    api! {
        client ApiPath {
            base https "example.com"
        }

        scope lol {
            path ["lol"]

            GET GetMatch(match_id: String)
            -> Json<()>
            {
                path ["matches", match_id]
            }
        }
    }

    use api_path::*;

    let (transport, h) = mock().reply(MockReply::ok_json(json_bytes(&()))).build();

    let api = ApiPath::new_with_transport(transport);

    api.request(endpoints::lol::GetMatch::new("a/b".to_string()))
        .execute()
        .await
        .unwrap();

    let reqs = h.recorded();
    assert_request(&reqs[0]).path("/lol/matches/a%2Fb");

    h.finish();
}

#[tokio::test]
async fn scope_path_part_builds_single_segment_and_encodes() {
    api! {
        client ApiPathFmt {
            base https "example.com"
        }

        GET One(v: String)
        -> Json<()>
        {
            path ["x", part["p", v]]
        }
    }

    use api_path_fmt::*;

    let (transport, h) = mock().reply(MockReply::ok_json(json_bytes(&()))).build();

    let api = ApiPathFmt::new_with_transport(transport);

    api.request(endpoints::One::new("a/b".to_string()))
        .execute()
        .await
        .unwrap();

    let reqs = h.recorded();
    assert_request(&reqs[0]).path("/x/pa%2Fb");

    h.finish();
}

#[tokio::test]
async fn scope_path_part_optional_omits_segment_when_missing() {
    api! {
        client ApiPathFmtOpt {
            base https "example.com"
        }

        GET One(v?: String)
        -> Json<()>
        {
            path ["x", part["p", v], "y"]
        }
    }

    use api_path_fmt_opt::*;

    let (transport, h) = mock()
        .replies([
            MockReply::ok_json(json_bytes(&())),
            MockReply::ok_json(json_bytes(&())),
        ])
        .build();

    let api = ApiPathFmtOpt::new_with_transport(transport);

    api.request(endpoints::One::new()).execute().await.unwrap();
    api.request(endpoints::One::new().v("z".to_string()))
        .execute()
        .await
        .unwrap();

    let reqs = h.recorded();
    assert_request(&reqs[0]).path("/x/y");
    assert_request(&reqs[1]).path("/x/pz/y");

    h.finish();
}

#[tokio::test]
async fn scope_path_optional_item_omitted_no_double_slash() {
    api! {
        client ApiOptSeg {
            base https "example.com"
        }

        GET One(opt?: String)
        -> Json<()>
        {
            path ["x", opt, "y"]
        }
    }

    use api_opt_seg::*;

    {
        let (transport, h) = mock().reply(MockReply::ok_json(json_bytes(&()))).build();

        let api = ApiOptSeg::new_with_transport(transport);
        api.request(endpoints::One::new()).execute().await.unwrap();

        let reqs = h.recorded();
        assert_request(&reqs[0]).path("/x/y");

        h.finish();
    }

    {
        let (transport, h) = mock().reply(MockReply::ok_json(json_bytes(&()))).build();

        let api = ApiOptSeg::new_with_transport(transport);
        api.request(endpoints::One::new().opt("z".to_string()))
            .execute()
            .await
            .unwrap();

        let reqs = h.recorded();
        assert_request(&reqs[0]).path("/x/z/y");

        h.finish();
    }
}

#[tokio::test]
async fn scope_host_part_adds_one_label() {
    api! {
        client ApiPrefixLayerFmt {
            base https "example.com"
        }

        scope layer(id: String) {
            host ["api", part["t", id]]

            GET One
            -> Json<()>
            {
                path ["x"]
            }
        }
    }

    use api_prefix_layer_fmt::*;

    let (transport, h) = mock().reply(MockReply::ok_json(json_bytes(&()))).build();

    let api = ApiPrefixLayerFmt::new_with_transport(transport);
    api.request(endpoints::layer::One::new("42".to_string()))
        .execute()
        .await
        .unwrap();

    let reqs = h.recorded();
    assert_request(&reqs[0])
        .host("api.t42.example.com")
        .path("/x");

    h.finish();
}

#[tokio::test]
async fn scope_host_part_optional_omits_label_when_missing() {
    api! {
        client ApiPrefixLayerFmtOpt {
            base https "example.com"
        }

        scope layer(id?: String) {
            host ["api", part["t", id]]

            GET One
            -> Json<()>
            {
                path ["x"]
            }
        }
    }

    use api_prefix_layer_fmt_opt::*;

    let (transport, h) = mock()
        .replies([
            MockReply::ok_json(json_bytes(&())),
            MockReply::ok_json(json_bytes(&())),
        ])
        .build();

    let api = ApiPrefixLayerFmtOpt::new_with_transport(transport);

    api.request(endpoints::layer::One::new())
        .execute()
        .await
        .unwrap();
    api.request(endpoints::layer::One::new().id("z".to_string()))
        .execute()
        .await
        .unwrap();

    let reqs = h.recorded();
    assert_request(&reqs[0]).host("api.example.com");
    assert_request(&reqs[1]).host("api.tz.example.com");

    h.finish();
}

#[tokio::test]
async fn scope_path_part_in_layer_builds_single_segment_and_encodes() {
    api! {
        client ApiPathLayerFmt {
            base https "example.com"
        }

        scope layer(v: String) {
            path ["api", part["p", v]]

            GET One
            -> Json<()>
            {
                path ["x"]
            }
        }
    }

    use api_path_layer_fmt::*;

    let (transport, h) = mock().reply(MockReply::ok_json(json_bytes(&()))).build();

    let api = ApiPathLayerFmt::new_with_transport(transport);

    api.request(endpoints::layer::One::new("a/b".to_string()))
        .execute()
        .await
        .unwrap();

    let reqs = h.recorded();
    assert_request(&reqs[0]).path("/api/pa%2Fb/x");

    h.finish();
}

#[tokio::test]
async fn scope_path_part_in_layer_optional_omits_segment_no_double_slash() {
    api! {
        client ApiPathLayerFmtOpt {
            base https "example.com"
        }

        scope layer(v?: String) {
            path ["api", part["p", v], "z"]

            GET One
            -> Json<()>
            {
                path ["x"]
            }
        }
    }

    use api_path_layer_fmt_opt::*;

    let (transport, h) = mock()
        .replies([
            MockReply::ok_json(json_bytes(&())),
            MockReply::ok_json(json_bytes(&())),
        ])
        .build();

    let api = ApiPathLayerFmtOpt::new_with_transport(transport);

    api.request(endpoints::layer::One::new())
        .execute()
        .await
        .unwrap();
    api.request(endpoints::layer::One::new().v("k".to_string()))
        .execute()
        .await
        .unwrap();

    let reqs = h.recorded();
    assert_request(&reqs[0]).path("/api/z/x");
    assert_request(&reqs[1]).path("/api/pk/z/x");

    h.finish();
}
