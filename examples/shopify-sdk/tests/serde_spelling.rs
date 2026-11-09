//! Frozen scalar spellings from the pre-migration formatter.
use concord_example_shopify_sdk::graphql::ErrorCode;

#[test]
fn scalar_error_codes_keep_every_wire_literal() {
    for (value, expected) in [
        (ErrorCode::Throttled, "THROTTLED"),
        (ErrorCode::AccessDenied, "ACCESS_DENIED"),
        (ErrorCode::ShopInactive, "SHOP_INACTIVE"),
        (ErrorCode::InternalServerError, "INTERNAL_SERVER_ERROR"),
        (ErrorCode::MaxCostExceeded, "MAX_COST_EXCEEDED"),
    ] {
        assert_eq!(value.to_string(), expected);
        assert_eq!(
            serde_json::to_string(&value).unwrap(),
            serde_json::to_string(expected).unwrap()
        );
    }
    assert_eq!(
        ErrorCode::Other("future/code".into()).to_string(),
        "future/code"
    );
}

#[test]
fn operation_enums_keep_every_wire_literal() {
    {
        use concord_example_shopify_sdk::ops::catalog::ProductStatus;
        for (value, expected) in [
            (ProductStatus::Active, "ACTIVE"),
            (ProductStatus::Archived, "ARCHIVED"),
            (ProductStatus::Draft, "DRAFT"),
            (ProductStatus::Unlisted, "UNLISTED"),
        ] {
            assert_eq!(value.to_string(), expected);
            assert_eq!(
                serde_json::to_string(&value).unwrap(),
                serde_json::to_string(expected).unwrap()
            );
        }
        assert_eq!(
            ProductStatus::Other("future/code".into()).to_string(),
            "future/code"
        );
    }
    {
        use concord_example_shopify_sdk::ops::catalog::CountPrecision;
        for (value, expected) in [
            (CountPrecision::Exact, "EXACT"),
            (CountPrecision::AtLeast, "AT_LEAST"),
        ] {
            assert_eq!(value.to_string(), expected);
            assert_eq!(
                serde_json::to_string(&value).unwrap(),
                serde_json::to_string(expected).unwrap()
            );
        }
        assert_eq!(
            CountPrecision::Other("future/code".into()).to_string(),
            "future/code"
        );
    }
    {
        use concord_example_shopify_sdk::ops::bulk::BulkOperationStatus;
        for (value, expected) in [
            (BulkOperationStatus::Created, "CREATED"),
            (BulkOperationStatus::Running, "RUNNING"),
            (BulkOperationStatus::Completed, "COMPLETED"),
            (BulkOperationStatus::Canceling, "CANCELING"),
            (BulkOperationStatus::Canceled, "CANCELED"),
            (BulkOperationStatus::Failed, "FAILED"),
            (BulkOperationStatus::Expired, "EXPIRED"),
        ] {
            assert_eq!(value.to_string(), expected);
            assert_eq!(
                serde_json::to_string(&value).unwrap(),
                serde_json::to_string(expected).unwrap()
            );
        }
        assert_eq!(
            BulkOperationStatus::Other("future/code".into()).to_string(),
            "future/code"
        );
    }
    {
        use concord_example_shopify_sdk::ops::bulk::BulkOperationErrorCode;
        for (value, expected) in [
            (BulkOperationErrorCode::AccessDenied, "ACCESS_DENIED"),
            (
                BulkOperationErrorCode::InternalServerError,
                "INTERNAL_SERVER_ERROR",
            ),
            (BulkOperationErrorCode::Timeout, "TIMEOUT"),
        ] {
            assert_eq!(value.to_string(), expected);
            assert_eq!(
                serde_json::to_string(&value).unwrap(),
                serde_json::to_string(expected).unwrap()
            );
        }
        assert_eq!(
            BulkOperationErrorCode::Other("future/code".into()).to_string(),
            "future/code"
        );
    }
    {
        use concord_example_shopify_sdk::ops::bulk::BulkOperationType;
        for (value, expected) in [
            (BulkOperationType::Query, "QUERY"),
            (BulkOperationType::Mutation, "MUTATION"),
        ] {
            assert_eq!(value.to_string(), expected);
            assert_eq!(
                serde_json::to_string(&value).unwrap(),
                serde_json::to_string(expected).unwrap()
            );
        }
        assert_eq!(
            BulkOperationType::Other("future/code".into()).to_string(),
            "future/code"
        );
    }
}
