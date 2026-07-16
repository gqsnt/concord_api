use std::collections::VecDeque;

use super::identity::{
    CanonicalIdentity, EncodingLimits, HeaderField, IdentityEncodingError, IdentityInput,
    deterministic_digest,
};
use super::limits::{CycleDetector, PaginationLimits};
use super::model::{
    AuthenticationMetadata, Controller, ControllerModel, Page, PaginationEngine, RequestModel,
    SideEffectCounters,
};
use super::{Boundary, PaginationError};

fn request(label: &str) -> RequestModel {
    RequestModel {
        endpoint_discriminator: b"catalog::list_widgets".to_vec(),
        method_token: b"GET".to_vec(),
        canonical_logical_url: format!("https://api.invalid/widgets?page={label}").into_bytes(),
        public_headers: vec![HeaderField::new("accept", b"application/test")],
        authentication: AuthenticationMetadata::inert(1),
    }
}

fn identity(request: &RequestModel) -> CanonicalIdentity {
    CanonicalIdentity::encode(request.identity_input()).expect("fixture identity must encode")
}

fn limits(max_pages: usize, max_items: usize, identity_bytes: usize) -> PaginationLimits {
    PaginationLimits {
        max_pages,
        max_items,
        max_retained_identity_bytes: identity_bytes,
    }
}

#[derive(Default)]
struct ScriptController {
    next_requests: VecDeque<RequestModel>,
    fail: bool,
}

impl ScriptController {
    fn continuing(requests: impl IntoIterator<Item = RequestModel>) -> Self {
        Self {
            next_requests: requests.into_iter().collect(),
            fail: false,
        }
    }
}

impl Controller for ScriptController {
    fn next(&mut self, _page: &Page) -> Result<Option<RequestModel>, PaginationError> {
        if self.fail {
            return Err(PaginationError::Controller);
        }
        Ok(self.next_requests.pop_front())
    }
}

mod pagination_identity {
    use super::*;

    #[test]
    fn pagination_identity_is_deterministic() {
        let first_instance = request("1");
        let second_instance = request("1");

        let first = identity(&first_instance);
        let second = identity(&second_instance);

        assert_eq!(first, second, "content-equal endpoint instances must match");
        assert_eq!(
            deterministic_digest(first.bytes()),
            deterministic_digest(second.bytes())
        );
    }

    #[test]
    fn pagination_identity_includes_endpoint_discriminator() {
        let left = request("1");
        let mut right = left.clone();
        right.endpoint_discriminator = b"catalog::search_widgets".to_vec();

        assert_ne!(identity(&left), identity(&right));
    }

    #[test]
    fn pagination_identity_includes_exact_method() {
        let upper = request("1");
        let mut different_verb = upper.clone();
        different_verb.method_token = b"POST".to_vec();
        let mut different_case = upper.clone();
        different_case.method_token = b"get".to_vec();

        assert_ne!(identity(&upper), identity(&different_verb));
        assert_ne!(identity(&upper), identity(&different_case));
    }

    #[test]
    fn pagination_identity_includes_canonical_url() {
        let left = request("1");
        let mut right = left.clone();
        right.canonical_logical_url = b"https://api.invalid/widgets?page=%31".to_vec();

        let left_identity = identity(&left);
        let right_identity = identity(&right);
        assert_ne!(left_identity, right_identity);

        let mut framed_url = vec![3];
        framed_url.extend_from_slice(
            &u64::try_from(left.canonical_logical_url.len())
                .expect("fixture length fits")
                .to_be_bytes(),
        );
        framed_url.extend_from_slice(&left.canonical_logical_url);
        assert!(
            left_identity
                .bytes()
                .windows(framed_url.len())
                .any(|window| window == framed_url),
            "the already-canonical logical URL must be included byte-for-byte"
        );
    }

    #[test]
    fn pagination_identity_preserves_query_order_through_canonical_url() {
        let mut left = request("1");
        left.canonical_logical_url = b"https://api.invalid/widgets?a=1&b=2".to_vec();
        let mut right = left.clone();
        right.canonical_logical_url = b"https://api.invalid/widgets?b=2&a=1".to_vec();

        assert_ne!(identity(&left), identity(&right));
    }

    #[test]
    fn pagination_identity_preserves_repeated_query_order_through_canonical_url() {
        let mut left = request("1");
        left.canonical_logical_url = b"https://api.invalid/widgets?a=1&a=2".to_vec();
        let mut right = left.clone();
        right.canonical_logical_url = b"https://api.invalid/widgets?a=2&a=1".to_vec();

        assert_ne!(identity(&left), identity(&right));
    }

    #[test]
    fn pagination_identity_has_no_redundant_query_representation() {
        let candidate = request("1");
        let input = IdentityInput {
            endpoint_discriminator: &candidate.endpoint_discriminator,
            method_token: &candidate.method_token,
            canonical_logical_url: &candidate.canonical_logical_url,
            public_headers: &candidate.public_headers,
        };

        assert_eq!(
            CanonicalIdentity::encode(input).expect("structural identity input must encode"),
            identity(&candidate)
        );
    }

    #[test]
    fn pagination_identity_canonicalizes_header_name_case() {
        let mut lower = request("1");
        lower.public_headers = vec![HeaderField::new("x-page", b"one")];
        let mut mixed = lower.clone();
        mixed.public_headers = vec![HeaderField::new("X-PaGe", b"one")];

        assert_eq!(identity(&lower), identity(&mixed));
    }

    #[test]
    fn pagination_identity_sorts_distinct_header_names() {
        let mut left = request("1");
        left.public_headers = vec![
            HeaderField::new("z-last", b"z"),
            HeaderField::new("a-first", b"a"),
        ];
        let mut right = left.clone();
        right.public_headers.reverse();

        assert_eq!(identity(&left), identity(&right));
    }

    #[test]
    fn pagination_identity_preserves_repeated_header_value_order() {
        let mut left = request("1");
        left.public_headers = vec![
            HeaderField::new("x-page", b"first"),
            HeaderField::new("X-PAGE", b"second"),
        ];
        let mut right = left.clone();
        right.public_headers.reverse();

        assert_ne!(identity(&left), identity(&right));
    }

    #[test]
    fn pagination_identity_preserves_exact_header_value_bytes() {
        let mut spaced = request("1");
        spaced.public_headers = vec![HeaderField::new("x-page", b" marker ")];
        let mut trimmed = spaced.clone();
        trimmed.public_headers = vec![HeaderField::new("x-page", b"marker")];

        assert_ne!(identity(&spaced), identity(&trimmed));
    }

    #[test]
    fn pagination_identity_encoding_is_unambiguous() {
        let mut left = request("1");
        left.endpoint_discriminator = b"ab".to_vec();
        left.method_token = b"c".to_vec();
        left.public_headers = vec![HeaderField::new("x-separator", b":|,\n\0")];

        let mut right = left.clone();
        right.endpoint_discriminator = b"a".to_vec();
        right.method_token = b"bc".to_vec();

        assert_ne!(identity(&left), identity(&right));
        assert_eq!(identity(&left), identity(&left.clone()));
    }

    #[test]
    fn pagination_identity_retained_bytes_count_canonical_components() {
        let candidate = request("1");
        let identity = identity(&candidate);
        let expected = candidate.endpoint_discriminator.len()
            + candidate.method_token.len()
            + candidate.canonical_logical_url.len()
            + candidate
                .public_headers
                .iter()
                .map(|header| header.name.len() + header.value.len())
                .sum::<usize>();

        assert_eq!(identity.retained_byte_len(), expected);
        assert!(
            identity.bytes().len() > identity.retained_byte_len(),
            "format tags and length prefixes are not canonical payload accounting"
        );
    }

    #[test]
    fn pagination_identity_retained_bytes_exclude_separate_query_accounting() {
        let mut without_query = request("1");
        without_query.canonical_logical_url = b"https://api.invalid/widgets".to_vec();
        let mut with_query = without_query.clone();
        with_query.canonical_logical_url = b"https://api.invalid/widgets?a=1&a=2".to_vec();

        let retained_delta = identity(&with_query)
            .retained_byte_len()
            .checked_sub(identity(&without_query).retained_byte_len())
            .expect("the query-bearing URL is longer");
        let url_delta =
            with_query.canonical_logical_url.len() - without_query.canonical_logical_url.len();

        assert_eq!(retained_delta, url_delta);
    }

    #[test]
    fn pagination_identity_excludes_authentication() {
        let left = request("1");
        let mut right = left.clone();
        right.authentication = AuthenticationMetadata::inert(2);
        assert_ne!(left.authentication, right.authentication);

        let left_identity = identity(&left);
        let right_identity = identity(&right);
        assert_eq!(left_identity.bytes(), right_identity.bytes());
        assert_eq!(
            deterministic_digest(left_identity.bytes()),
            deterministic_digest(right_identity.bytes())
        );

        let mut detector = CycleDetector::default();
        detector
            .check_and_retain(left_identity, usize::MAX)
            .expect("first identity is accepted");
        assert_eq!(
            detector.check_and_retain(right_identity, usize::MAX),
            Err(PaginationError::RepeatedIdentity)
        );
    }

    #[test]
    fn pagination_identity_rejects_invalid_header_names() {
        for invalid in [b"".as_slice(), b"bad name", b"x:\xff", "x-é".as_bytes()] {
            let mut candidate = request("1");
            candidate.public_headers = vec![HeaderField::new(invalid, b"marker")];
            assert_eq!(
                CanonicalIdentity::encode(candidate.identity_input()),
                Err(IdentityEncodingError::InvalidHeaderName),
                "invalid header name bytes: {invalid:?}"
            );
        }
    }

    #[test]
    fn pagination_identity_rejects_immediate_repeat() {
        let a = identity(&request("A"));
        let mut detector = CycleDetector::default();
        detector
            .check_and_retain(a.clone(), usize::MAX)
            .expect("first A is accepted");
        assert_eq!(
            detector.check_and_retain(a, usize::MAX),
            Err(PaginationError::RepeatedIdentity)
        );
    }

    #[test]
    fn pagination_identity_rejects_two_cycle() {
        let mut detector = CycleDetector::default();
        for label in ["A", "B"] {
            detector
                .check_and_retain(identity(&request(label)), usize::MAX)
                .unwrap_or_else(|error| panic!("{label} should be accepted: {error:?}"));
        }
        assert_eq!(
            detector.check_and_retain(identity(&request("A")), usize::MAX),
            Err(PaginationError::RepeatedIdentity)
        );
    }

    #[test]
    fn pagination_identity_rejects_longer_cycle() {
        let mut detector = CycleDetector::default();
        for label in ["A", "B", "C"] {
            detector
                .check_and_retain(identity(&request(label)), usize::MAX)
                .unwrap_or_else(|error| panic!("{label} should be accepted: {error:?}"));
        }
        assert_eq!(
            detector.check_and_retain(identity(&request("B")), usize::MAX),
            Err(PaginationError::RepeatedIdentity)
        );
    }

    #[test]
    fn pagination_identity_hash_collision_uses_full_equality() {
        fn constant_digest(_: &[u8]) -> u64 {
            7
        }

        let a = identity(&request("A"));
        let b = identity(&request("B"));
        let expected_bytes = a
            .retained_byte_len()
            .checked_add(b.retained_byte_len())
            .expect("fixture byte total fits");
        let mut detector = CycleDetector::new(constant_digest);

        detector
            .check_and_retain(a.clone(), usize::MAX)
            .expect("A is accepted");
        detector
            .check_and_retain(b, usize::MAX)
            .expect("unequal B is accepted despite the digest collision");
        assert_eq!(detector.retained_identity_count(), 2);
        assert_eq!(detector.retained_identity_bytes(), expected_bytes);
        assert_eq!(
            detector.check_and_retain(a, usize::MAX),
            Err(PaginationError::RepeatedIdentity)
        );
    }
}

mod pagination_bounds {
    use super::*;

    #[test]
    fn pagination_cycle_rejected_before_side_effects() {
        let a = request("A");
        let mut engine = PaginationEngine::new(limits(2, 10, usize::MAX), vec![]);
        engine
            .preload_identity(&a)
            .expect("fixture identity is preloaded");
        let retained_count = engine.retained_identity_count();
        let retained_bytes = engine.retained_identity_bytes();

        let result = engine.run(a, ControllerModel::Custom(ScriptController::default()));

        assert_eq!(result, Err(PaginationError::RepeatedIdentity));
        assert_eq!(engine.counters, SideEffectCounters::default());
        assert_eq!(engine.retained_identity_count(), retained_count);
        assert_eq!(engine.retained_identity_bytes(), retained_bytes);
    }

    #[test]
    fn pagination_identity_limit_rejected_before_retention() {
        let a = request("A");
        let identity_len = identity(&a).retained_byte_len();
        let mut engine = PaginationEngine::new(limits(2, 10, identity_len - 1), vec![]);

        let result = engine.run(a, ControllerModel::Custom(ScriptController::default()));

        assert_eq!(result, Err(PaginationError::RetainedIdentityByteLimit));
        assert_eq!(engine.retained_identity_count(), 0);
        assert_eq!(engine.retained_identity_bytes(), 0);
    }

    #[test]
    fn pagination_identity_limit_rejected_before_side_effects() {
        let a = request("A");
        let mut engine = PaginationEngine::new(limits(2, 10, 0), vec![]);

        let result = engine.run(a, ControllerModel::Custom(ScriptController::default()));

        assert_eq!(result, Err(PaginationError::RetainedIdentityByteLimit));
        assert_eq!(engine.counters, SideEffectCounters::default());
    }

    #[test]
    fn pagination_retained_identity_overflow_fails_closed() {
        let a = request("A");
        let mut engine = PaginationEngine::new(limits(2, 10, usize::MAX), vec![]);
        engine.set_retained_identity_bytes(usize::MAX);

        let result = engine.run(a, ControllerModel::Custom(ScriptController::default()));

        assert_eq!(result, Err(PaginationError::RetainedIdentityByteOverflow));
        assert_eq!(engine.retained_identity_count(), 0);
        assert_eq!(engine.retained_identity_bytes(), usize::MAX);
        assert_eq!(engine.counters, SideEffectCounters::default());
    }

    #[test]
    fn pagination_page_limit_rejected_before_first_request() {
        let mut engine = PaginationEngine::new(limits(0, 10, usize::MAX), vec![]);

        let result = engine.run(
            request("A"),
            ControllerModel::Custom(ScriptController::default()),
        );

        assert_eq!(result, Err(PaginationError::PageLimit));
        assert_eq!(engine.retained_identity_count(), 0);
        assert_eq!(engine.retained_identity_bytes(), 0);
        assert_eq!(engine.counters, SideEffectCounters::default());
    }

    #[test]
    fn pagination_page_limit_rejected_before_controller() {
        let mut engine =
            PaginationEngine::new(limits(1, 10, usize::MAX), vec![Page { items: vec![1] }]);
        let controller = ScriptController::continuing([request("B")]);

        let result = engine.run(request("A"), ControllerModel::Custom(controller));

        assert_eq!(result, Err(PaginationError::PageLimit));
        assert_eq!(engine.page_count(), 1);
        assert_eq!(engine.counters.controller, 0);
        assert_eq!(engine.counters.transport, 1);
        assert_eq!(engine.retained_identity_count(), 1);
    }

    #[test]
    fn pagination_page_limit_rejected_after_multiple_pages() {
        let mut engine = PaginationEngine::new(
            limits(2, 10, usize::MAX),
            vec![Page { items: vec![1] }, Page { items: vec![2] }],
        );
        let controller = ScriptController::continuing([request("B"), request("C")]);

        let result = engine.run(request("A"), ControllerModel::Custom(controller));

        assert_eq!(result, Err(PaginationError::PageLimit));
        assert_eq!(engine.page_count(), 2);
        assert_eq!(engine.counters.controller, 1);
        assert_eq!(engine.counters.transport, 2);
        assert_eq!(engine.retained_identity_count(), 2);
    }

    #[test]
    fn pagination_item_limit_rejected_before_append() {
        let mut engine = PaginationEngine::new(
            limits(2, 2, usize::MAX),
            vec![Page {
                items: vec![1, 2, 3],
            }],
        );

        let result = engine.run(
            request("A"),
            ControllerModel::Custom(ScriptController::default()),
        );

        assert_eq!(result, Err(PaginationError::ItemLimit));
        assert!(engine.aggregate().is_empty());
        assert_eq!(engine.item_count(), 0);
        assert_eq!(engine.page_count(), 0);
        assert_eq!(engine.counters.transport, 1);
        assert_eq!(engine.counters.controller, 0);
    }

    #[test]
    fn pagination_item_exact_limit_append_succeeds() {
        let mut engine =
            PaginationEngine::new(limits(2, 2, usize::MAX), vec![Page { items: vec![1, 2] }]);

        let result = engine.run(
            request("A"),
            ControllerModel::Custom(ScriptController::default()),
        );

        assert_eq!(result, Ok(vec![1, 2]));
        assert_eq!(engine.item_count(), 2);
        assert_eq!(engine.page_count(), 1);
        assert_eq!(engine.counters.controller, 1);
    }

    #[test]
    fn pagination_item_overflow_rejected_before_append() {
        let mut engine = PaginationEngine::new(
            limits(2, usize::MAX, usize::MAX),
            vec![Page { items: vec![1] }],
        );
        engine.set_item_count(usize::MAX);

        let result = engine.run(
            request("A"),
            ControllerModel::Custom(ScriptController::default()),
        );

        assert_eq!(result, Err(PaginationError::ItemCountOverflow));
        assert!(engine.aggregate().is_empty());
        assert_eq!(engine.item_count(), usize::MAX);
        assert_eq!(engine.counters.controller, 0);
    }

    #[test]
    fn pagination_identity_byte_overflow_fails_closed() {
        let mut engine = PaginationEngine::new(limits(2, 10, usize::MAX), vec![]);
        engine.set_encoding_limits(EncodingLimits {
            max_field_length: 1,
            max_total_length: usize::MAX,
        });

        let result = engine.run(
            request("A"),
            ControllerModel::Custom(ScriptController::default()),
        );

        assert_eq!(
            result,
            Err(PaginationError::IdentityEncoding(
                IdentityEncodingError::EncodedFieldLengthOverflow
            ))
        );
        assert_eq!(engine.retained_identity_count(), 0);
        assert_eq!(engine.counters, SideEffectCounters::default());
    }

    #[test]
    fn pagination_identity_total_byte_overflow_fails_closed() {
        let mut engine = PaginationEngine::new(limits(2, 10, usize::MAX), vec![]);
        engine.set_encoding_limits(EncodingLimits {
            max_field_length: u64::MAX,
            max_total_length: 1,
        });

        let result = engine.run(
            request("A"),
            ControllerModel::Custom(ScriptController::default()),
        );

        assert_eq!(
            result,
            Err(PaginationError::IdentityEncoding(
                IdentityEncodingError::CanonicalIdentityLengthOverflow
            ))
        );
        assert_eq!(engine.retained_identity_count(), 0);
        assert_eq!(engine.counters, SideEffectCounters::default());
    }

    #[test]
    fn pagination_page_count_overflow_fails_closed() {
        let mut engine = PaginationEngine::new(limits(usize::MAX, 10, usize::MAX), vec![]);
        engine.set_page_count(usize::MAX);

        let result = engine.run(
            request("A"),
            ControllerModel::Custom(ScriptController::default()),
        );

        assert_eq!(result, Err(PaginationError::PageCountOverflow));
        assert_eq!(engine.retained_identity_count(), 0);
        assert_eq!(engine.counters, SideEffectCounters::default());
    }

    #[test]
    fn pagination_boundary_counter_overflow_fails_closed() {
        let mut engine = PaginationEngine::new(limits(2, 10, usize::MAX), vec![]);
        engine.counters.hook = usize::MAX;

        let result = engine.run(
            request("A"),
            ControllerModel::Custom(ScriptController::default()),
        );

        assert_eq!(
            result,
            Err(PaginationError::BoundaryCountOverflow(Boundary::Hook))
        );
        assert_eq!(engine.counters.hook, usize::MAX);
        assert_eq!(engine.counters.credential, 0);
        assert_eq!(engine.counters.limiter, 0);
        assert_eq!(engine.counters.transport, 0);
        assert_eq!(engine.retained_identity_count(), 1);
    }

    #[test]
    fn pagination_each_boundary_counter_overflow_is_checked() {
        for boundary in [
            Boundary::Credential,
            Boundary::Limiter,
            Boundary::Transport,
            Boundary::Controller,
        ] {
            let mut engine =
                PaginationEngine::new(limits(2, 10, usize::MAX), vec![Page { items: vec![1] }]);
            match boundary {
                Boundary::Credential => engine.counters.credential = usize::MAX,
                Boundary::Limiter => engine.counters.limiter = usize::MAX,
                Boundary::Transport => engine.counters.transport = usize::MAX,
                Boundary::Controller => engine.counters.controller = usize::MAX,
                Boundary::Hook => unreachable!("hook has a focused test"),
            }

            let result = engine.run(
                request("A"),
                ControllerModel::Custom(ScriptController::default()),
            );

            assert_eq!(
                result,
                Err(PaginationError::BoundaryCountOverflow(boundary)),
                "boundary counter must fail explicitly: {boundary:?}"
            );
            match boundary {
                Boundary::Credential => {
                    assert_eq!(engine.counters.hook, 1);
                    assert_eq!(engine.counters.limiter, 0);
                    assert_eq!(engine.counters.transport, 0);
                }
                Boundary::Limiter => {
                    assert_eq!(engine.counters.hook, 1);
                    assert_eq!(engine.counters.credential, 1);
                    assert_eq!(engine.counters.transport, 0);
                }
                Boundary::Transport => {
                    assert_eq!(engine.counters.hook, 1);
                    assert_eq!(engine.counters.credential, 1);
                    assert_eq!(engine.counters.limiter, 1);
                }
                Boundary::Controller => {
                    assert_eq!(engine.counters.transport, 1);
                    assert_eq!(engine.aggregate(), &[1]);
                }
                Boundary::Hook => unreachable!("hook has a focused test"),
            }
        }
    }

    #[test]
    fn pagination_returns_no_partial_result_on_failure() {
        let mut engine = PaginationEngine::new(
            limits(3, 3, usize::MAX),
            vec![Page { items: vec![1, 2] }, Page { items: vec![3, 4] }],
        );
        let controller = ScriptController::continuing([request("B")]);

        let result = engine.run(request("A"), ControllerModel::Custom(controller));

        assert_eq!(result, Err(PaginationError::ItemLimit));
        assert_eq!(engine.aggregate(), &[1, 2]);
        assert_eq!(engine.item_count(), 2);
        assert_eq!(engine.page_count(), 1);
        assert_eq!(engine.counters.transport, 2);
        assert_eq!(engine.counters.controller, 1);
    }
}

mod pagination_controller {
    use super::*;

    #[test]
    fn custom_controller_continues_after_short_page() {
        let mut engine = PaginationEngine::new(
            limits(3, 10, usize::MAX),
            vec![Page { items: vec![1] }, Page { items: vec![2, 3] }],
        );
        let controller = ScriptController::continuing([request("B")]);

        let result = engine.run(request("A"), ControllerModel::Custom(controller));

        assert_eq!(result, Ok(vec![1, 2, 3]));
        assert_eq!(engine.counters.transport, 2);
        assert_eq!(engine.counters.controller, 2);
        assert_eq!(engine.page_count(), 2);
    }

    #[test]
    fn private_builtin_controller_stops_after_short_page() {
        let mut engine = PaginationEngine::new(
            limits(3, 10, usize::MAX),
            vec![Page { items: vec![1] }, Page { items: vec![2, 3] }],
        );
        let controller = ControllerModel::BuiltIn {
            inner: ScriptController::continuing([request("B")]),
            requested_page_size: 2,
        };

        let result = engine.run(request("A"), controller);

        assert_eq!(result, Ok(vec![1]));
        assert_eq!(engine.counters.transport, 1);
        assert_eq!(engine.counters.controller, 0);
        assert_eq!(engine.page_count(), 1);
    }

    #[test]
    fn custom_controller_has_no_capacity_hint() {
        struct NextOnlyController;

        impl Controller for NextOnlyController {
            fn next(&mut self, _page: &Page) -> Result<Option<RequestModel>, PaginationError> {
                Ok(None)
            }
        }

        let mut engine =
            PaginationEngine::new(limits(2, 10, usize::MAX), vec![Page { items: vec![1] }]);

        let result = engine.run(request("A"), ControllerModel::Custom(NextOnlyController));

        assert_eq!(result, Ok(vec![1]));
        assert_eq!(engine.counters.controller, 1);
    }
}
