// Client lifecycle phase modules intentionally share one private parent namespace.
use super::*;

pub(super) struct PublicRequestHead {
    pub(super) meta: RequestMeta,
    pub(super) url: url::Url,
    pub(super) headers: http::HeaderMap,
    pub(super) timeout: Option<std::time::Duration>,
    pub(super) retry: RetrySetting,
    pub(super) rate_limit: RateLimitPlan,
    pub(super) auth_plan: crate::auth::AuthPlacementPlan,
    pub(super) reserved_headers: Vec<http::HeaderName>,
}

impl PublicRequestHead {
    pub(super) fn apply_auth_preflight(
        &mut self,
        auth_plan: &crate::auth::AuthPlacementPlan,
        ctx: &ErrorContext,
    ) -> Result<(), ApiClientError> {
        auth_plan
            .validate_public_request_with_reserved_headers(
                &self.headers,
                &self.url,
                &self.reserved_headers,
            )
            .map_err(|source| ApiClientError::Auth {
                ctx: ctx.clone(),
                source,
            })?;
        self.auth_plan = auth_plan.clone();
        Ok(())
    }

    pub(super) fn finish(
        self,
        body: crate::io::ProducedBody,
        ctx: &ErrorContext,
    ) -> Result<BuiltRequest, ApiClientError> {
        let uri = self.url.as_str().parse::<http::Uri>().map_err(|_| {
            ApiClientError::PolicyViolation {
                ctx: ctx.clone(),
                msg: "resolved request URI is invalid",
            }
        })?;
        let mut headers = self.headers;
        let (terminal_body, terminal_media_type) =
            body.into_legacy_transport_parts()
                .map_err(|_| ApiClientError::PolicyViolation {
                    ctx: ctx.clone(),
                    msg: "request body compatibility materialization failed",
                })?;
        crate::io::apply_attempt_media_type(&mut headers, terminal_media_type.as_ref()).map_err(
            |()| ApiClientError::PolicyViolation {
                ctx: ctx.clone(),
                msg: "request Content-Type conflicts with produced body media type",
            },
        )?;
        let mut message = http::Request::new(terminal_body);
        *message.method_mut() = self.meta.method.clone();
        *message.uri_mut() = uri;
        *message.version_mut() = http::Version::HTTP_11;
        *message.headers_mut() = headers;
        message
            .extensions_mut()
            .insert(crate::transport::RequestExecutionContext {
                meta: self.meta,
                timeout: self.timeout,
            });
        message.extensions_mut().insert(self.auth_plan);
        Ok(BuiltRequest {
            message,
            retry: self.retry,
            rate_limit: self.rate_limit,
        })
    }
}

impl<Cx: ClientContext, T: Transport> ApiClient<Cx, T> {
    pub(super) fn resolve_public_request_head(
        &self,
        plan: &crate::endpoint::RequestPlanView,
        body: &crate::io::PreparedBody,
        meta: RequestMeta,
    ) -> Result<PublicRequestHead, ApiClientError> {
        let ctx = ErrorContext {
            endpoint: plan.endpoint.meta.name,
            method: plan.endpoint.meta.method.clone(),
        };
        let timeout = plan.overrides.timeout.or(plan.endpoint.policy.timeout);
        let retry = plan.endpoint.policy.retry.clone();
        let rate_limit = plan.endpoint.policy.rate_limit.clone();
        let configured_retry_idempotency = match &retry {
            RetrySetting::Config(config) => Some(&config.idempotency),
            RetrySetting::Inherit | RetrySetting::Off => None,
        };
        if let Some(crate::retry::RetryIdempotency::Header(name)) = configured_retry_idempotency
            && !crate::header_ownership::is_retry_idempotency_exception_allowed(name)
        {
            return Err(ApiClientError::PolicyViolation {
                ctx: ctx.clone(),
                msg: "configured retry idempotency header is not permitted",
            });
        }

        let base = format!(
            "{}://{}",
            plan.endpoint.route.scheme, plan.endpoint.route.host
        );
        let mut url = url::Url::parse(&base).map_err(|e| ApiClientError::BuildUrl {
            ctx: ctx.clone(),
            source: e,
        })?;
        url.set_path(&plan.endpoint.route.path);
        if !plan.endpoint.policy.query.is_empty() {
            let mut qp = url.query_pairs_mut();
            for (k, v) in &plan.endpoint.policy.query {
                qp.append_pair(k, v);
            }
        }

        let public_header_error = |source: crate::header_ownership::HeaderOwnershipError,
                                   scope: &'static str| {
            let scope = match scope {
                "client-wide" => "credential-bearing client-wide header",
                _ => "credential-bearing endpoint header",
            };
            if crate::redaction::is_credential_bearing_header_name(source.header_name().as_str()) {
                ApiClientError::Auth {
                    ctx: ctx.clone(),
                    source: crate::auth::AuthError::new(
                        crate::auth::AuthErrorKind::InvalidConfiguration,
                        format!("{scope} collides with authentication ownership"),
                    ),
                }
            } else {
                ApiClientError::PolicyViolation {
                    ctx: ctx.clone(),
                    msg: "public API headers include a transport-reserved header",
                }
            }
        };
        let mut retry_idempotency = None;
        if let RetrySetting::Config(config) = &retry
            && let crate::retry::RetryIdempotency::Header(name) = &config.idempotency
        {
            retry_idempotency = Some(name.clone());
        }
        let retry_idempotency_exceptions = retry_idempotency.into_iter().collect::<Vec<_>>();
        crate::header_ownership::validate_public_headers_with_exceptions(
            &self.api_headers,
            &retry_idempotency_exceptions,
        )
        .map_err(|source| public_header_error(source, "client-wide"))?;
        crate::header_ownership::validate_public_headers_with_exceptions(
            &plan.endpoint.policy.headers,
            &retry_idempotency_exceptions,
        )
        .map_err(|source| public_header_error(source, "endpoint"))?;
        // Resolve client-wide runtime headers first; endpoint headers retain
        // the established replacement semantics.
        let mut headers = self.api_headers.clone();
        for name in plan.endpoint.policy.headers.keys() {
            headers.remove(name);
        }
        for (name, value) in &plan.endpoint.policy.headers {
            headers.append(name.clone(), value.clone());
        }
        // These are origin-facing headers, deliberately not Reqwest defaults.
        headers
            .entry(http::header::ACCEPT)
            .or_insert_with(|| http::HeaderValue::from_static("*/*"));
        headers.insert(
            http::header::USER_AGENT,
            http::HeaderValue::from_static(concat!("concord/", env!("CARGO_PKG_VERSION"))),
        );
        crate::io::apply_prepared_body_media_type(&mut headers, body).map_err(|()| {
            ApiClientError::PolicyViolation {
                ctx: ctx.clone(),
                msg: "request Content-Type conflicts with prepared body media type",
            }
        })?;
        let mut reserved_headers = Vec::new();
        if body.reserves_content_type() || body.media_type().is_some() {
            reserved_headers.push(http::header::CONTENT_TYPE);
        }
        if let RetrySetting::Config(config) = &retry
            && let crate::retry::RetryIdempotency::Header(name) = &config.idempotency
        {
            reserved_headers.push(name.clone());
        }
        Ok(PublicRequestHead {
            meta,
            url,
            headers,
            timeout,
            retry,
            rate_limit,
            auth_plan: crate::auth::AuthPlacementPlan::default(),
            reserved_headers,
        })
    }

    pub(super) fn produce_attempt_body(
        &self,
        body: &mut crate::io::PreparedBody,
        ctx: &ErrorContext,
    ) -> Result<crate::io::ProducedBody, ApiClientError> {
        body.produce_for_attempt().map_err(|error| {
            let msg = match error.kind() {
                crate::io::BodyProductionErrorKind::AlreadyConsumed => {
                    "one-shot request body has already been consumed"
                }
                crate::io::BodyProductionErrorKind::FactoryFailure => "request body factory failed",
            };
            ApiClientError::PolicyViolation {
                ctx: ctx.clone(),
                msg,
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct BuildCx;

    impl ClientContext for BuildCx {
        type Vars = ();
        type AuthVars = ();
        type AuthState = ();
        const SCHEME: http::uri::Scheme = http::uri::Scheme::HTTPS;
        const DOMAIN: &'static str = "example.test";

        fn init_auth_state(_: &Self::Vars, _: &Self::AuthVars) -> Self::AuthState {}
    }

    fn header_test_plan(headers: http::HeaderMap) -> crate::endpoint::RequestPlanView {
        crate::endpoint::RequestPlanView {
            endpoint: crate::endpoint::EndpointPlan {
                meta: crate::endpoint::EndpointMeta {
                    name: "HeaderOwnership",
                    method: http::Method::GET,
                    idempotent: true,
                    facade_path: &[],
                },
                route: crate::endpoint::ResolvedRoute::new(
                    http::uri::Scheme::HTTPS,
                    "example.test",
                    "/headers",
                ),
                policy: crate::policy::ResolvedPolicy {
                    headers,
                    ..Default::default()
                },
                response: crate::endpoint::ResponsePlan {
                    accept: None,
                    no_content: false,
                    format: crate::codec::Format::Text,
                },
                pagination: None,
            },
            overrides: crate::endpoint::RequestOverrides::default(),
        }
    }

    #[test]
    fn client_headers_merge_before_endpoint_headers_and_concord_materializes_defaults() {
        let mut client_headers = http::HeaderMap::new();
        client_headers.insert("x-shared", http::HeaderValue::from_static("client"));
        client_headers.insert("x-client", http::HeaderValue::from_static("present"));
        let client = ApiClient::<BuildCx>::new((), ())
            .with_api_headers(client_headers)
            .expect("safe client headers");

        let mut endpoint_headers = http::HeaderMap::new();
        endpoint_headers.append("x-shared", http::HeaderValue::from_static("endpoint-a"));
        endpoint_headers.append("x-shared", http::HeaderValue::from_static("endpoint-b"));
        let head = client
            .resolve_public_request_head(
                &header_test_plan(endpoint_headers),
                &crate::io::PreparedBody::empty(),
                RequestMeta {
                    endpoint: "HeaderOwnership",
                    method: http::Method::GET,
                    idempotent: true,
                    attempt: 0,
                    page_index: 0,
                },
            )
            .expect("header preparation");

        assert_eq!(
            head.headers.get("x-client"),
            Some(&http::HeaderValue::from_static("present"))
        );
        let shared = head.headers.get_all("x-shared");
        assert_eq!(shared.iter().count(), 2);
        assert_eq!(
            head.headers.get(http::header::ACCEPT),
            Some(&http::HeaderValue::from_static("*/*"))
        );
        assert_eq!(
            head.headers.get(http::header::USER_AGENT),
            Some(&http::HeaderValue::from_static(concat!(
                "concord/",
                env!("CARGO_PKG_VERSION")
            )))
        );
    }

    #[test]
    fn protocol_headers_are_rejected_at_client_and_endpoint_boundaries() {
        let mut forbidden = http::HeaderMap::new();
        forbidden.insert(
            http::header::HOST,
            http::HeaderValue::from_static("elsewhere.test"),
        );
        assert!(
            ApiClient::<BuildCx>::new((), ())
                .with_api_headers(forbidden)
                .is_err()
        );

        let mut endpoint_headers = http::HeaderMap::new();
        endpoint_headers.insert(
            http::header::ACCEPT_ENCODING,
            http::HeaderValue::from_static("gzip"),
        );
        let result = ApiClient::<BuildCx>::new((), ()).resolve_public_request_head(
            &header_test_plan(endpoint_headers),
            &crate::io::PreparedBody::empty(),
            RequestMeta {
                endpoint: "HeaderOwnership",
                method: http::Method::GET,
                idempotent: true,
                attempt: 0,
                page_index: 0,
            },
        );
        let Err(error) = result else {
            panic!("transport-owned endpoint header must fail");
        };
        assert!(matches!(error, ApiClientError::PolicyViolation { .. }));
    }

    #[test]
    fn credential_bearing_endpoint_headers_fail_before_authentication_preflight() {
        let mut endpoint_headers = http::HeaderMap::new();
        endpoint_headers.insert(
            http::header::AUTHORIZATION,
            http::HeaderValue::from_static("Bearer public-secret"),
        );
        let result = ApiClient::<BuildCx>::new((), ()).resolve_public_request_head(
            &header_test_plan(endpoint_headers),
            &crate::io::PreparedBody::empty(),
            RequestMeta {
                endpoint: "HeaderOwnership",
                method: http::Method::GET,
                idempotent: true,
                attempt: 0,
                page_index: 0,
            },
        );
        assert!(matches!(result, Err(ApiClientError::Auth { .. })));
    }

    #[test]
    fn configured_idempotency_header_is_reserved_before_authentication() {
        let mut plan = header_test_plan(http::HeaderMap::new());
        plan.endpoint.policy.retry = RetrySetting::Config(crate::retry::RetryConfig {
            max_attempts: 2,
            idempotency: crate::retry::RetryIdempotency::Header(http::HeaderName::from_static(
                "x-retry-attempt-key",
            )),
            ..Default::default()
        });
        let mut head = ApiClient::<BuildCx>::new((), ())
            .resolve_public_request_head(
                &plan,
                &crate::io::PreparedBody::empty(),
                RequestMeta {
                    endpoint: "HeaderOwnership",
                    method: http::Method::POST,
                    idempotent: false,
                    attempt: 0,
                    page_index: 0,
                },
            )
            .expect("request head");
        let auth_plan = crate::auth::AuthPlacementPlan::from_auth_plan(&crate::auth::AuthPlan {
            requirements: vec![crate::auth::AuthRequirement {
                credential: crate::auth::CredentialRef {
                    id: crate::auth::CredentialId::new("test", "token"),
                },
                placement: crate::auth::AuthPlacement::Header("x-retry-attempt-key"),
                usage_id: crate::auth::AuthUsageId::new("idempotency"),
                step_id: None,
                provenance: crate::auth::AuthProvenance::default(),
                challenge: Default::default(),
            }],
        })
        .expect("placement plan");
        let error_ctx = ErrorContext {
            endpoint: "HeaderOwnership",
            method: http::Method::POST,
        };
        assert!(head.apply_auth_preflight(&auth_plan, &error_ctx).is_err());
    }

    #[test]
    fn configured_idempotency_header_with_suffix_key_is_accepted_as_public_and_reserved() {
        let configured = http::HeaderName::from_static("request-idempotency-key");
        let mut plan = header_test_plan(http::HeaderMap::new());
        plan.endpoint.policy.headers.insert(
            configured.clone(),
            http::HeaderValue::from_static("stable-transport-key"),
        );
        plan.endpoint.policy.retry = RetrySetting::Config(crate::retry::RetryConfig {
            max_attempts: 2,
            idempotency: crate::retry::RetryIdempotency::Header(configured.clone()),
            ..Default::default()
        });
        let head = ApiClient::<BuildCx>::new((), ())
            .resolve_public_request_head(
                &plan,
                &crate::io::PreparedBody::empty(),
                RequestMeta {
                    endpoint: "HeaderOwnershipIdempotentSuffix",
                    method: http::Method::POST,
                    idempotent: false,
                    attempt: 0,
                    page_index: 0,
                },
            )
            .expect("public request head should be prepared");
        assert_eq!(
            head.headers.get(&configured),
            Some(&http::HeaderValue::from_static("stable-transport-key"))
        );
        assert_eq!(head.reserved_headers, vec![configured.clone()]);
        let auth_plan = crate::auth::AuthPlacementPlan::from_auth_plan(&crate::auth::AuthPlan {
            requirements: vec![crate::auth::AuthRequirement {
                credential: crate::auth::CredentialRef {
                    id: crate::auth::CredentialId::new("test", "token"),
                },
                placement: crate::auth::AuthPlacement::Header("request-idempotency-key"),
                usage_id: crate::auth::AuthUsageId::new("idempotency"),
                step_id: None,
                provenance: crate::auth::AuthProvenance::default(),
                challenge: Default::default(),
            }],
        })
        .unwrap_err();
        assert!(format!("{auth_plan}").contains("reserved"));
    }

    #[test]
    fn configured_idempotency_header_is_validated_before_other_header_preflight() {
        for name in [
            "authorization",
            "host",
            "content-length",
            "content-type",
            "user-agent",
            "proxy-authorization",
            "cookie",
            "set-cookie",
            "www-authenticate",
            "x-api-key",
            "x-client-key",
            "key",
        ] {
            let mut plan = header_test_plan(http::HeaderMap::new());
            plan.endpoint.policy.retry = RetrySetting::Config(crate::retry::RetryConfig {
                max_attempts: 2,
                idempotency: crate::retry::RetryIdempotency::Header(http::HeaderName::from_static(
                    name,
                )),
                ..Default::default()
            });
            let result = ApiClient::<BuildCx>::new((), ()).resolve_public_request_head(
                &plan,
                &crate::io::PreparedBody::empty(),
                RequestMeta {
                    endpoint: "ConfiguredRetryIdempotency",
                    method: http::Method::POST,
                    idempotent: false,
                    attempt: 0,
                    page_index: 0,
                },
            );
            let Err(error) = result else {
                panic!("invalid configured retry idempotency should fail: {name}");
            };
            assert!(
                matches!(error, ApiClientError::PolicyViolation { .. }),
                "unexpected error for {name}: {error:?}"
            );
        }

        let configured = http::HeaderName::from_static("idempotency-key");
        let mut plan = header_test_plan(http::HeaderMap::new());
        plan.endpoint.policy.retry = RetrySetting::Config(crate::retry::RetryConfig {
            max_attempts: 2,
            idempotency: crate::retry::RetryIdempotency::Header(configured.clone()),
            ..Default::default()
        });
        let head = ApiClient::<BuildCx>::new((), ())
            .resolve_public_request_head(
                &plan,
                &crate::io::PreparedBody::empty(),
                RequestMeta {
                    endpoint: "ConfiguredRetryIdempotency",
                    method: http::Method::POST,
                    idempotent: false,
                    attempt: 0,
                    page_index: 0,
                },
            )
            .expect("configured idempotency header should be allowed");
        assert_eq!(
            head.reserved_headers,
            vec![http::HeaderName::from_static("idempotency-key")]
        );
    }

    #[test]
    fn configured_request_identity_headers_are_permitted_as_retry_headers() {
        for name in ["idempotency-key", "request-idempotency-key", "x-request-id"] {
            let configured = http::HeaderName::from_static(name);
            let mut plan = header_test_plan(http::HeaderMap::new());
            plan.endpoint.policy.retry = RetrySetting::Config(crate::retry::RetryConfig {
                max_attempts: 2,
                idempotency: crate::retry::RetryIdempotency::Header(configured.clone()),
                ..Default::default()
            });
            let head = ApiClient::<BuildCx>::new((), ())
                .resolve_public_request_head(
                    &plan,
                    &crate::io::PreparedBody::empty(),
                    RequestMeta {
                        endpoint: "RequestIdentityRetryHeader",
                        method: http::Method::POST,
                        idempotent: false,
                        attempt: 0,
                        page_index: 0,
                    },
                )
                .expect("request identity headers must remain valid retry configuration");
            assert_eq!(head.reserved_headers, vec![configured.clone()]);
            assert_eq!(
                head.headers.get(&configured),
                None,
                "request identity retry header alone must not inject a default value"
            );
        }
    }

    #[test]
    fn client_api_headers_and_endpoint_headers_use_correct_scope_in_public_header_errors() {
        let endpoint_error = ApiClient::<BuildCx>::new((), ()).resolve_public_request_head(
            &header_test_plan({
                let mut headers = http::HeaderMap::new();
                headers.insert(
                    http::header::AUTHORIZATION,
                    http::HeaderValue::from_static("endpoint"),
                );
                headers
            }),
            &crate::io::PreparedBody::empty(),
            RequestMeta {
                endpoint: "EndpointScope",
                method: http::Method::GET,
                idempotent: true,
                attempt: 0,
                page_index: 0,
            },
        );
        let err = match endpoint_error {
            Ok(_) => panic!("endpoint forbidden header"),
            Err(err) => err,
        };
        let rendered = format!("{err}");
        assert!(rendered.contains("endpoint header"));
        assert!(!rendered.contains("authorization"));

        let client = ApiClient::<BuildCx>::new((), ());
        let mut client = client;
        client.api_headers.insert(
            http::header::AUTHORIZATION,
            http::HeaderValue::from_static("public"),
        );
        let client_error = client.resolve_public_request_head(
            &header_test_plan(http::HeaderMap::new()),
            &crate::io::PreparedBody::empty(),
            RequestMeta {
                endpoint: "ClientScope",
                method: http::Method::GET,
                idempotent: true,
                attempt: 0,
                page_index: 0,
            },
        );
        let err = match client_error {
            Ok(_) => panic!("client wide forbidden header"),
            Err(err) => err,
        };
        let rendered = format!("{err}");
        assert!(rendered.contains("client-wide header"));
        assert!(!rendered.contains("authorization"));
    }

    #[test]
    fn set_api_headers_rejects_credential_bearing_names_without_revealing_raw_name() {
        let mut client = ApiClient::<BuildCx>::new((), ());
        let mut headers = http::HeaderMap::new();
        headers.insert(
            http::HeaderName::from_static("x-api-key"),
            http::HeaderValue::from_static("public"),
        );
        let err = client
            .set_api_headers(headers)
            .expect_err("credential-bearing header");
        let rendered = format!("{err}");
        let debug = format!("{err:?}");
        assert!(!rendered.contains("x-api-key"));
        assert!(!debug.contains("x-api-key"));
        assert!(rendered.contains("credential"));
    }

    #[cfg(feature = "default-tls")]
    #[test]
    fn with_reqwest_builder_fallible_rejects_invalid_pem_without_leaking_sentinels() {
        let marker = "BUILD_PEM_SENTINEL";
        let pem = format!(
            "-----BEGIN CERTIFICATE-----\n{marker}\nnot-base64-content\n-----END CERTIFICATE-----"
        );
        let result = ApiClient::<BuildCx>::with_reqwest_builder_fallible((), (), move |builder| {
            builder.add_trusted_root_pem(pem.as_bytes())
        });
        let error = match result {
            Ok(_) => panic!("invalid cert must fail"),
            Err(error) => error,
        };
        let diagnostics = format!("{error}\n{:?}", error);
        assert!(!diagnostics.contains(marker), "{diagnostics}");
    }

    #[test]
    fn with_reqwest_builder_fallible_is_accessible_with_infallible_configuration() {
        let _client = ApiClient::<BuildCx>::with_reqwest_builder_fallible((), (), |builder| {
            Ok(builder.connect_timeout(std::time::Duration::from_millis(50)))
        })
        .expect("infallible reqwest configuration should remain ergonomic");
    }

    #[test]
    fn collision_preflight_leaves_one_shot_body_unconsumed() {
        let mut headers = http::HeaderMap::new();
        headers.insert(
            http::header::AUTHORIZATION,
            http::HeaderValue::from_static("public"),
        );
        let mut head = PublicRequestHead {
            meta: RequestMeta {
                endpoint: "PreflightOneShot",
                method: http::Method::POST,
                idempotent: false,
                attempt: 0,
                page_index: 0,
            },
            url: "https://example.com/items".parse().expect("url"),
            headers,
            timeout: None,
            retry: RetrySetting::Off,
            rate_limit: RateLimitPlan::new(),
            auth_plan: Default::default(),
            reserved_headers: Vec::new(),
        };
        let auth_plan = crate::auth::AuthPlacementPlan::from_auth_plan(&crate::auth::AuthPlan {
            requirements: vec![crate::auth::AuthRequirement {
                credential: crate::auth::CredentialRef {
                    id: crate::auth::CredentialId::new("test", "token"),
                },
                placement: crate::auth::AuthPlacement::Bearer,
                usage_id: crate::auth::AuthUsageId::new("preflight-use"),
                step_id: Some("preflight"),
                provenance: crate::auth::AuthProvenance::new("preflight"),
                challenge: Default::default(),
            }],
        })
        .expect("placement plan");
        let ctx = ErrorContext {
            endpoint: "PreflightOneShot",
            method: http::Method::POST,
        };
        let mut body = crate::io::PreparedBody::one_shot(crate::body::DynBody::empty(), None);

        head.apply_auth_preflight(&auth_plan, &ctx)
            .expect_err("collision must fail");
        assert!(body.produce_for_attempt().is_ok());
    }
}
